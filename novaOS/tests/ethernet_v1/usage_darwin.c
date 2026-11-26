#include <libproc.h>
#include <stdint.h>
#include <sys/resource.h>
#include <unistd.h>

struct usage_eth { uint64_t cpu_ns, cycles, rss, peak, switches, wake_irq; };

static uint64_t timeval_ns(struct timeval t) {
    return (uint64_t)t.tv_sec * 1000000000ULL + (uint64_t)t.tv_usec * 1000ULL;
}

int ethernet_usage(struct usage_eth *out) {
    struct rusage_info_v4 info = {0};
    struct rusage ru = {0};
    if (proc_pid_rusage(getpid(), RUSAGE_INFO_V4, (rusage_info_t *)&info) != 0 ||
        getrusage(RUSAGE_SELF, &ru) != 0)
        return -1;
    out->cpu_ns = timeval_ns(ru.ru_utime) + timeval_ns(ru.ru_stime);
    out->cycles = info.ri_cycles;
    out->rss = info.ri_resident_size;
    out->peak = ru.ru_maxrss;
    out->switches = ru.ru_nvcsw + ru.ru_nivcsw;
    out->wake_irq = info.ri_interrupt_wkups;
    return 0;
}
