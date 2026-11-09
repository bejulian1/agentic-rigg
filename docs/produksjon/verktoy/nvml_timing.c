/*
 * nvml_timing.c – måler oppstart og pollekostnad for NVML. BARE LESING.
 *
 * Kaller bare nvmlInit_v2, nvmlShutdown, nvmlDeviceGetHandleByIndex_v2 og
 * nvmlDeviceGet*. Ingen Set-kall er deklarert i fila.
 *
 * Bygg:  gcc -O2 -o ~/.cache/rigg-build/nvml_timing nvml_timing.c -ldl
 * Bruk:  nvml_timing                 (oppstart + 50 pollerunder, tett)
 *        nvml_timing watch MS SEK    (poll hvert MS ms i SEK sekunder og
 *                                     skriv P-tilstand, klokke og effekt,
 *                                     for å se om pollingen vekker kortet)
 *        nvml_timing calls N         (N runder med 1 s pause, tid per kall)
 *        nvml_timing lean MS SEK     (slank runde med 10 kall; kjør under
 *                                     /usr/bin/time for å se CPU-tiden)
 *
 * Merk: standardmodusen poller tett og vekker kortet (P8 → P0) i noen
 * sekunder. Det er ufarlig, men gir P0-tall. «calls» og «watch 1000» gir
 * tallene i ro.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

typedef int ret_t;
typedef void *dev_t_;
typedef struct { unsigned int gpu, memory; } util_t;
typedef struct { unsigned long long total, free, used; } mem_t;

static double now_us(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e6 + t.tv_nsec / 1e3;
}

static long rss_kb(void) {
    FILE *f = fopen("/proc/self/status", "r");
    char line[256];
    long kb = -1;
    if (!f) return -1;
    while (fgets(line, sizeof line, f))
        if (strncmp(line, "VmRSS:", 6) == 0) { kb = atol(line + 6); break; }
    fclose(f);
    return kb;
}

#define SYM(name, type) __typeof__(type) name = (__typeof__(type))dlsym(h, #name); if (!name) { fprintf(stderr, "mangler %s\n", #name); return 1; }

int main(int argc, char **argv) {
    long rss0 = rss_kb();
    double t0 = now_us();
    void *h = dlopen("libnvidia-ml.so.1", RTLD_NOW);
    double t_dl = now_us() - t0;
    if (!h) { fprintf(stderr, "dlopen: %s\n", dlerror()); return 1; }

    SYM(nvmlInit_v2, ret_t (*)(void));
    SYM(nvmlShutdown, ret_t (*)(void));
    SYM(nvmlDeviceGetHandleByIndex_v2, ret_t (*)(unsigned, dev_t_ *));
    SYM(nvmlDeviceGetName, ret_t (*)(dev_t_, char *, unsigned));
    SYM(nvmlDeviceGetTemperature, ret_t (*)(dev_t_, int, unsigned *));
    SYM(nvmlDeviceGetPowerUsage, ret_t (*)(dev_t_, unsigned *));
    SYM(nvmlDeviceGetTotalEnergyConsumption, ret_t (*)(dev_t_, unsigned long long *));
    SYM(nvmlDeviceGetClockInfo, ret_t (*)(dev_t_, int, unsigned *));
    SYM(nvmlDeviceGetUtilizationRates, ret_t (*)(dev_t_, util_t *));
    SYM(nvmlDeviceGetEncoderUtilization, ret_t (*)(dev_t_, unsigned *, unsigned *));
    SYM(nvmlDeviceGetDecoderUtilization, ret_t (*)(dev_t_, unsigned *, unsigned *));
    SYM(nvmlDeviceGetMemoryInfo, ret_t (*)(dev_t_, mem_t *));
    SYM(nvmlDeviceGetPerformanceState, ret_t (*)(dev_t_, int *));
    SYM(nvmlDeviceGetFanSpeed_v2, ret_t (*)(dev_t_, unsigned, unsigned *));
    SYM(nvmlDeviceGetTargetFanSpeed, ret_t (*)(dev_t_, unsigned, unsigned *));
    SYM(nvmlDeviceGetCurrPcieLinkGeneration, ret_t (*)(dev_t_, unsigned *));
    SYM(nvmlDeviceGetCurrPcieLinkWidth, ret_t (*)(dev_t_, unsigned *));
    SYM(nvmlDeviceGetCurrentClocksEventReasons, ret_t (*)(dev_t_, unsigned long long *));
    SYM(nvmlDeviceGetEnforcedPowerLimit, ret_t (*)(dev_t_, unsigned *));

    long rss_dl = rss_kb();
    t0 = now_us();
    ret_t r = nvmlInit_v2();
    double t_init = now_us() - t0;
    if (r) { fprintf(stderr, "nvmlInit_v2 = %d\n", r); return 1; }
    long rss_init = rss_kb();
    dev_t_ d;
    t0 = now_us();
    nvmlDeviceGetHandleByIndex_v2(0, &d);
    double t_handle = now_us() - t0;
    char name[96];
    t0 = now_us();
    nvmlDeviceGetName(d, name, sizeof name);
    double t_name = now_us() - t0;

    unsigned temp, pw, c[4], f0, f1, tg, pg, pwid, enc, encp, dec, decp, plim;
    unsigned long long en, reasons;
    int ps;
    util_t ut;
    mem_t mem;

#define ROUND()                                              \
    do {                                                     \
        nvmlDeviceGetTemperature(d, 0, &temp);               \
        nvmlDeviceGetPowerUsage(d, &pw);                     \
        nvmlDeviceGetTotalEnergyConsumption(d, &en);         \
        for (int k = 0; k < 4; k++) nvmlDeviceGetClockInfo(d, k, &c[k]); \
        nvmlDeviceGetUtilizationRates(d, &ut);               \
        nvmlDeviceGetEncoderUtilization(d, &enc, &encp);     \
        nvmlDeviceGetDecoderUtilization(d, &dec, &decp);     \
        nvmlDeviceGetMemoryInfo(d, &mem);                    \
        nvmlDeviceGetPerformanceState(d, &ps);               \
        nvmlDeviceGetFanSpeed_v2(d, 0, &f0);                 \
        nvmlDeviceGetFanSpeed_v2(d, 1, &f1);                 \
        nvmlDeviceGetTargetFanSpeed(d, 0, &tg);              \
        nvmlDeviceGetCurrPcieLinkGeneration(d, &pg);         \
        nvmlDeviceGetCurrPcieLinkWidth(d, &pwid);            \
        nvmlDeviceGetCurrentClocksEventReasons(d, &reasons); \
        nvmlDeviceGetEnforcedPowerLimit(d, &plim);           \
    } while (0)

    /* «calls N»: N runder med ett sekunds pause, tid per kall (ms). Viser
       hva hvert kall koster når kortet står i ro (P8) og ikke vekkes. */
    if (argc >= 3 && strcmp(argv[1], "calls") == 0) {
        int n = atoi(argv[2]);
#define T(label, expr) do { double a_ = now_us(); expr; printf("%s=%.2f ", label, (now_us() - a_) / 1000); } while (0)
        for (int i = 0; i < n; i++) {
            nvmlDeviceGetPerformanceState(d, &ps);
            printf("P%d: ", ps);
            T("temp", nvmlDeviceGetTemperature(d, 0, &temp));
            T("power", nvmlDeviceGetPowerUsage(d, &pw));
            T("energy", nvmlDeviceGetTotalEnergyConsumption(d, &en));
            T("clk_gr", nvmlDeviceGetClockInfo(d, 0, &c[0]));
            T("clk_sm", nvmlDeviceGetClockInfo(d, 1, &c[1]));
            T("clk_mem", nvmlDeviceGetClockInfo(d, 2, &c[2]));
            T("clk_vid", nvmlDeviceGetClockInfo(d, 3, &c[3]));
            T("util", nvmlDeviceGetUtilizationRates(d, &ut));
            T("enc", nvmlDeviceGetEncoderUtilization(d, &enc, &encp));
            T("dec", nvmlDeviceGetDecoderUtilization(d, &dec, &decp));
            T("mem", nvmlDeviceGetMemoryInfo(d, &mem));
            T("pstate", nvmlDeviceGetPerformanceState(d, &ps));
            T("fan0", nvmlDeviceGetFanSpeed_v2(d, 0, &f0));
            T("fan1", nvmlDeviceGetFanSpeed_v2(d, 1, &f1));
            T("target0", nvmlDeviceGetTargetFanSpeed(d, 0, &tg));
            T("pcie_gen", nvmlDeviceGetCurrPcieLinkGeneration(d, &pg));
            T("pcie_w", nvmlDeviceGetCurrPcieLinkWidth(d, &pwid));
            T("reasons", nvmlDeviceGetCurrentClocksEventReasons(d, &reasons));
            T("plimit", nvmlDeviceGetEnforcedPowerLimit(d, &plim));
            printf("\n");
            fflush(stdout);
            sleep(1);
        }
        nvmlShutdown();
        return 0;
    }

    /* «lean MS SEK»: en slank runde uten klokkehendelser, energi, SM-/videoklokke,
       encoder/decoder og viftemål. Skriver bare samlet tid; bruk /usr/bin/time
       for CPU-tid. */
    if (argc >= 4 && strcmp(argv[1], "lean") == 0) {
        int ms = atoi(argv[2]), sec = atoi(argv[3]);
        int n = sec * 1000 / ms;
        double sum = 0;
        for (int i = 0; i < n; i++) {
            double a = now_us();
            nvmlDeviceGetTemperature(d, 0, &temp);
            nvmlDeviceGetPowerUsage(d, &pw);
            nvmlDeviceGetClockInfo(d, 0, &c[0]);
            nvmlDeviceGetClockInfo(d, 2, &c[2]);
            nvmlDeviceGetUtilizationRates(d, &ut);
            nvmlDeviceGetMemoryInfo(d, &mem);
            nvmlDeviceGetPerformanceState(d, &ps);
            nvmlDeviceGetFanSpeed_v2(d, 0, &f0);
            nvmlDeviceGetFanSpeed_v2(d, 1, &f1);
            nvmlDeviceGetCurrPcieLinkGeneration(d, &pg);
            double b = now_us();
            sum += b - a;
            double left = ms * 1000.0 - (now_us() - a);
            if (left > 0) usleep((useconds_t)left);
        }
        printf("lean: %d runder, snitt %.2f ms, siste P%d %u MHz %.1f W\n", n, sum / n / 1000, ps, c[0], pw / 1000.0);
        nvmlShutdown();
        return 0;
    }

    if (argc >= 4 && strcmp(argv[1], "watch") == 0) {
        int ms = atoi(argv[2]), sec = atoi(argv[3]);
        int n = sec * 1000 / ms;
        printf("t_s\tpstate\tgr_mhz\tmem_mhz\tpower_w\tgpu_pct\tround_ms\n");
        double start = now_us();
        for (int i = 0; i < n; i++) {
            double a = now_us();
            ROUND();
            double b = now_us();
            printf("%.1f\tP%d\t%u\t%u\t%.1f\t%u\t%.2f\n", (a - start) / 1e6, ps, c[0], c[2], pw / 1000.0, ut.gpu, (b - a) / 1000);
            fflush(stdout);
            double left = ms * 1000.0 - (now_us() - a);
            if (left > 0) usleep((useconds_t)left);
        }
        nvmlShutdown();
        return 0;
    }

    double first0 = now_us();
    ROUND();
    double t_first = now_us() - first0;
    double rounds[50];
    for (int i = 0; i < 50; i++) {
        double a = now_us();
        ROUND();
        rounds[i] = now_us() - a;
    }
    double t_sd0 = now_us();
    nvmlShutdown();
    double t_sd = now_us() - t_sd0;
    /* median */
    for (int i = 0; i < 50; i++)
        for (int j = i + 1; j < 50; j++)
            if (rounds[j] < rounds[i]) { double x = rounds[i]; rounds[i] = rounds[j]; rounds[j] = x; }
    printf("{\"gpu\":\"%s\",\"dlopen_ms\":%.2f,\"init_ms\":%.2f,\"handle_ms\":%.2f,\"name_us\":%.1f,"
           "\"first_round_ms\":%.2f,\"round_ms_median\":%.2f,\"round_ms_min\":%.2f,\"round_ms_max\":%.2f,"
           "\"shutdown_ms\":%.2f,\"calls_per_round\":19,\"rss_kb\":{\"start\":%ld,\"dlopen\":%ld,\"init\":%ld},\"sample\":{\"temp\":%u,\"power_mw\":%u,\"gr\":%u,\"mem\":%u,\"pstate\":%d}}\n",
           name, t_dl / 1e3, t_init / 1e3, t_handle / 1e3, t_name, t_first / 1e3, rounds[25] / 1e3, rounds[0] / 1e3,
           rounds[49] / 1e3, t_sd / 1e3, rss0, rss_dl, rss_init, temp, pw, c[0], c[2], ps);
    return 0;
}
