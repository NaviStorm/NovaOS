#include <libproc.h>
#include <mach/mach_time.h>
#include <stdint.h>
#include <sys/resource.h>
#include <unistd.h>

struct usage_eth { uint64_t cpu_ns, cycles, rss, peak, switches, wake_irq; };

int ethernet_usage(struct usage_eth *out) {
    struct rusage_info_v4 info = {0};
    struct rusage ru = {0};
    mach_timebase_info_data_t timebase;
    if (proc_pid_rusage(getpid(), RUSAGE_INFO_V4, (rusage_info_t *)&info) != 0 ||
        getrusage(RUSAGE_SELF, &ru) != 0 || mach_timebase_info(&timebase) != KERN_SUCCESS)
        return -1;
    out->cpu_ns = (uint64_t)(((__uint128_t)info.ri_user_time + info.ri_system_time)
                             * timebase.numer / timebase.denom);
    out->cycles = info.ri_cycles;
    out->rss = info.ri_resident_size;
    out->peak = ru.ru_maxrss;
    out->switches = ru.ru_nvcsw + ru.ru_nivcsw;
    out->wake_irq = info.ri_interrupt_wkups;
    return 0;
}
