// Preloaded into QEMU by the simulation runners (scripts/nodump.sh): a simulator crash then ends
// the process without a core dump, so systemd-coredump records nothing and the desktop shows no
// crash notice. The exit status (SIGSEGV) is unchanged, so runners still see and retry it.
#include <sys/prctl.h>
__attribute__((constructor)) static void nodump(void) { prctl(PR_SET_DUMPABLE, 0); }
