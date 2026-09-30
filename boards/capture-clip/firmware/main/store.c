// clip_store_t on LittleFS: copy-on-write, so a file is either its old or its new content after
// a power cut, and data is on flash once fsync returns.
#include "app.h"

#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#include "esp_littlefs.h"

#define PARTITION "storage"
#define SMALL_FILE_MAX (256 * 1024) // .meta and .ans are a few KB

void store_path(const char *name, char *out, size_t len)
{
    snprintf(out, len, STORE_DIR "/%s", name);
}

static bool st_list(void *ctx, void (*each)(void *arg, const char *name), void *arg)
{
    (void)ctx;
    DIR *dir = opendir(STORE_DIR);
    if (!dir) {
        return false;
    }
    for (struct dirent *entry; (entry = readdir(dir)) != NULL;) {
        each(arg, entry->d_name);
    }
    closedir(dir);
    return true;
}

static bool st_read(void *ctx, const char *name, char **data, size_t *len)
{
    (void)ctx;
    char path[80];
    struct stat info;
    store_path(name, path, sizeof path);
    if (stat(path, &info) != 0 || info.st_size > SMALL_FILE_MAX) {
        return false;
    }
    int fd = open(path, O_RDONLY);
    char *out = fd >= 0 ? malloc((size_t)info.st_size + 1) : NULL;
    bool ok = out && read(fd, out, (size_t)info.st_size) == (ssize_t)info.st_size;
    if (fd >= 0) {
        close(fd);
    }
    if (!ok) {
        free(out);
        return false;
    }
    out[info.st_size] = 0;
    *data = out;
    *len = (size_t)info.st_size;
    return true;
}

static bool st_write(void *ctx, const char *name, const char *data, size_t len)
{
    (void)ctx;
    char path[80];
    store_path(name, path, sizeof path);
    int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) {
        return false;
    }
    bool ok = write(fd, data, len) == (ssize_t)len && fsync(fd) == 0;
    return close(fd) == 0 && ok;
}

static bool st_rename(void *ctx, const char *from, const char *to)
{
    (void)ctx;
    char a[80], b[80];
    store_path(from, a, sizeof a);
    store_path(to, b, sizeof b);
    return rename(a, b) == 0;
}

static bool st_remove(void *ctx, const char *name)
{
    (void)ctx;
    char path[80];
    store_path(name, path, sizeof path);
    return unlink(path) == 0 || errno == ENOENT;
}

static int64_t st_size(void *ctx, const char *name)
{
    (void)ctx;
    char path[80];
    struct stat info;
    store_path(name, path, sizeof path);
    return stat(path, &info) == 0 ? (int64_t)info.st_size : -1;
}

static void *st_open(void *ctx, const char *name)
{
    (void)ctx;
    char path[80];
    store_path(name, path, sizeof path);
    int fd = open(path, O_RDONLY);
    return fd < 0 ? NULL : (void *)(intptr_t)(fd + 1);
}

static size_t st_read_at(void *ctx, void *file, int64_t offset, uint8_t *buf, size_t cap)
{
    (void)ctx;
    int fd = (int)(intptr_t)file - 1;
    if (lseek(fd, (off_t)offset, SEEK_SET) < 0) {
        return 0;
    }
    ssize_t got = read(fd, buf, cap);
    return got > 0 ? (size_t)got : 0;
}

static void st_close(void *ctx, void *file)
{
    (void)ctx;
    close((int)(intptr_t)file - 1);
}

static const clip_store_t store = {
    .list = st_list,
    .read = st_read,
    .write = st_write,
    .rename = st_rename,
    .remove = st_remove,
    .size = st_size,
    .open = st_open,
    .read_at = st_read_at,
    .close = st_close,
};

const clip_store_t *store_get(void)
{
    return &store;
}

bool store_mount(void)
{
    // A partition that does not mount is formatted: that is the first boot after flashing. (A
    // LittleFS that was once good does not stop mounting after a power cut.)
    esp_vfs_littlefs_conf_t conf = {.base_path = STORE_DIR, .partition_label = PARTITION, .format_if_mount_failed = true};
    esp_err_t err = esp_vfs_littlefs_register(&conf);
    if (err != ESP_OK) {
        say("store", "\"ok\":false,\"error\":\"%s\"", esp_err_to_name(err));
        return false;
    }
    return true;
}

int64_t store_free_bytes(void)
{
    size_t total = 0, used = 0;
    if (esp_littlefs_info(PARTITION, &total, &used) != ESP_OK) {
        return -1;
    }
    return (int64_t)total - (int64_t)used;
}
