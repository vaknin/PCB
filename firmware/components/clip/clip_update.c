// Firmware updates over Wi-Fi, the part that is only text and arithmetic: reading the manifest
// and deciding whether to install what it names. The app downloads, checks the SHA-256, switches
// slots and restarts; the new firmware marks itself good only after its own check passes, and
// the bootloader goes back to the old one otherwise.
#include "clip.h"

#include <stdlib.h>
#include <string.h>

static bool version_ok(const char *text, size_t len)
{
    if (len == 0) {
        return false;
    }
    for (size_t i = 0; i < len; i++) {
        char c = text[i];
        if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c && strchr("._+-", c)))) {
            return false;
        }
    }
    return true;
}

static int nibble(char c)
{
    return c >= '0' && c <= '9' ? c - '0' : c >= 'a' && c <= 'f' ? c - 'a' + 10 : c >= 'A' && c <= 'F' ? c - 'A' + 10 : -1;
}

bool clip_manifest_parse(const char *text, clip_manifest_t *manifest)
{
    *manifest = (clip_manifest_t){0};
    int seen = 0;
    for (const char *line = text; *line;) {
        const char *end = strchr(line, '\n'), *next = end ? end + 1 : line + strlen(line);
        if (!end) {
            end = next;
        }
        if (end > line && end[-1] == '\r') {
            end--;
        }
        const char *eq = memchr(line, '=', (size_t)(end - line));
        if (*line != '#' && eq) {
            size_t key = (size_t)(eq - line), len = (size_t)(end - eq - 1);
            const char *value = eq + 1;
            if (key == 7 && memcmp(line, "version", 7) == 0) {
                if (len >= sizeof manifest->version || !version_ok(value, len)) {
                    return false;
                }
                memcpy(manifest->version, value, len);
                manifest->version[len] = 0;
                seen |= 1;
            } else if (key == 3 && memcmp(line, "url", 3) == 0) {
                bool http = (len > 7 && memcmp(value, "http://", 7) == 0) || (len > 8 && memcmp(value, "https://", 8) == 0);
                if (len >= sizeof manifest->url || !http || memchr(value, ' ', len)) {
                    return false;
                }
                memcpy(manifest->url, value, len);
                manifest->url[len] = 0;
                seen |= 2;
            } else if (key == 4 && memcmp(line, "size", 4) == 0) {
                long long size = 0;
                for (size_t i = 0; i < len; i++) {
                    if (value[i] < '0' || value[i] > '9' || size > 0x7fffffff / 10 - 1) {
                        return false;
                    }
                    size = size * 10 + (value[i] - '0');
                }
                if (len == 0 || size <= 0) {
                    return false;
                }
                manifest->size = (long)size;
                seen |= 4;
            } else if (key == 6 && memcmp(line, "sha256", 6) == 0) {
                if (len != 64) {
                    return false;
                }
                for (int i = 0; i < 32; i++) {
                    int hi = nibble(value[2 * i]), lo = nibble(value[2 * i + 1]);
                    if (hi < 0 || lo < 0) {
                        return false;
                    }
                    manifest->sha256[i] = (uint8_t)(hi << 4 | lo);
                }
                seen |= 8;
            }
        }
        line = next;
    }
    return seen == 15;
}

clip_update_t clip_update_decide(const clip_manifest_t *manifest, const char *running, const char *rolled_back,
                                 long slot_bytes)
{
    if (strcmp(manifest->version, running) == 0) {
        return CLIP_UPDATE_NONE;
    }
    if (rolled_back && *rolled_back && strcmp(manifest->version, rolled_back) == 0) {
        return CLIP_UPDATE_KNOWN_BAD;
    }
    if (manifest->size > slot_bytes) {
        return CLIP_UPDATE_TOO_BIG;
    }
    return CLIP_UPDATE_INSTALL;
}
