/* Généré par scripts/generate_abi.py depuis abi/syscalls.toml. Modifier les sources. */
#ifndef NOVA_ABI_V1_H
#define NOVA_ABI_V1_H
#include <stdint.h>
#include <stddef.h>
#ifdef __cplusplus
#define NOVA_ABI_ASSERT static_assert
#define NOVA_ABI_ALIGN alignof
#else
#define NOVA_ABI_ASSERT _Static_assert
#define NOVA_ABI_ALIGN _Alignof
#endif

#define NOVA_SYS_PROCESS_ID UINT64_C(0x10001)
#define NOVA_SYS_THREAD_ID UINT64_C(0x10002)
#define NOVA_SYS_THREAD_YIELD UINT64_C(0x10003)
#define NOVA_SYS_PROCESS_EXIT UINT64_C(0x10004)
#define NOVA_SYS_PROCESS_WAIT UINT64_C(0x10005)
#define NOVA_SYS_DEBUG_WRITE UINT64_C(0x10006)
#define NOVA_SYS_IPC_CREATE UINT64_C(0x10020)
#define NOVA_SYS_IPC_DUP UINT64_C(0x10021)
#define NOVA_SYS_IPC_CLOSE UINT64_C(0x10022)
#define NOVA_SYS_IPC_REVOKE UINT64_C(0x10023)
#define NOVA_SYS_IPC_SEND UINT64_C(0x10024)
#define NOVA_SYS_IPC_RECEIVE UINT64_C(0x10025)
#define NOVA_SYS_IPC_GIVE UINT64_C(0x10026)
#define NOVA_SYS_IPC_NOTIFY UINT64_C(0x10027)
#define NOVA_SYS_IPC_CANCEL UINT64_C(0x10028)
#define NOVA_SYS_IPC_BOOTSTRAP UINT64_C(0x10029)
#define NOVA_SYS_IPC_PUT UINT64_C(0x1002c)
#define NOVA_SYS_IPC_TAKE UINT64_C(0x1002d)
#define NOVA_SYS_IPC_FEATURES UINT64_C(0x1003b)
#define NOVA_SYS_PORTAL UINT64_C(0x10040)
#define NOVA_SYS_HEAP_RESIZE UINT64_C(0x10041)
#define NOVA_SYS_SPAWN UINT64_C(0x10042)
#define NOVA_SYS_CONSOLE UINT64_C(0x10043)
#define NOVA_SYS_CLOCK_UTC UINT64_C(0x10044)
#define NOVA_SYS_PROCESS_SAMPLE UINT64_C(0x10045)
#define NOVA_SYS_PAGE_PORTAL UINT64_C(0x10046)
#define NOVA_SYS_THREAD_CREATE UINT64_C(0x10060)
#define NOVA_SYS_THREAD_EXIT UINT64_C(0x10061)
#define NOVA_SYS_THREAD_JOIN UINT64_C(0x10062)
#define NOVA_SYS_THREAD_DETACH UINT64_C(0x10063)
#define NOVA_SYS_THREAD_CANCEL UINT64_C(0x10064)
#define NOVA_SYS_THREAD_CANCELLED UINT64_C(0x10065)
#define NOVA_SYS_THREAD_TLS UINT64_C(0x10066)
#define NOVA_SYS_THREAD_WAIT UINT64_C(0x10067)
#define NOVA_SYS_THREAD_NOTIFY UINT64_C(0x10068)
#define NOVA_SYS_INFO UINT64_C(0x10070)
#define NOVA_SYS_WORKER UINT64_C(0x10071)
#define NOVA_SYS_STOP UINT64_C(0x10072)
#define NOVA_SYS_OFFER_STOP UINT64_C(0x10073)
#define NOVA_SYS_WATCH UINT64_C(0x10074)
#define NOVA_SYS_OFFERED UINT64_C(0x10075)
#define NOVA_SYS_TMP_CAP 0x10076
#define NOVA_SYS_TMP_CALL 0x10077
#define NOVA_SYS_TMP_WORKER 0x10078
#define NOVA_SYS_TMP_NEXT 0x10079
#define NOVA_SYS_TMP_COMPLETE 0x1007A
#define NOVA_SYS_SERVICE_PROCESS UINT64_C(0x1007b)
#define NOVA_SYS_CACHE_CONTROL UINT64_C(0x1007c)
#define NOVA_VERSION UINT32_C(1)
#define NOVA_OK UINT64_C(0)
#define NOVA_BAD_VERSION UINT64_C(1)
#define NOVA_UNKNOWN_CALL UINT64_C(2)
#define NOVA_BAD_ARGUMENT UINT64_C(3)
#define NOVA_BAD_ADDRESS UINT64_C(4)
#define NOVA_DENIED UINT64_C(5)
#define NOVA_LIMIT UINT64_C(6)
#define NOVA_SEND UINT32_C(1)
#define NOVA_RECEIVE UINT32_C(2)
#define NOVA_GRANT UINT32_C(4)
#define NOVA_MANAGE UINT32_C(8)
#define NOVA_WOULD_BLOCK UINT64_C(7)
#define NOVA_TIMED_OUT UINT64_C(8)
#define NOVA_CANCELLED UINT64_C(9)
#define NOVA_REVOKED UINT64_C(10)
#define NOVA_CLOSED UINT64_C(11)
#define NOVA_REFUSED UINT64_C(12)
#define NOVA_NO_MEMORY UINT64_C(13)
#define NOVA_IPC_FEATURE_PAGE UINT64_C(1)

typedef struct nova_Reply {
    uint64_t status;
    uint64_t value;
} nova_Reply;
NOVA_ABI_ASSERT(sizeof(nova_Reply) == 16, "Reply size");
NOVA_ABI_ASSERT(NOVA_ABI_ALIGN(nova_Reply) == 8, "Reply alignment");

typedef struct nova_WaitInfo {
    uint32_t version;
    uint32_t size;
    uint64_t process;
    uint64_t kind;
    uint64_t code;
} nova_WaitInfo;
NOVA_ABI_ASSERT(sizeof(nova_WaitInfo) == 32, "WaitInfo size");
NOVA_ABI_ASSERT(NOVA_ABI_ALIGN(nova_WaitInfo) == 8, "WaitInfo alignment");

typedef struct nova_BulkV1 {
    uint32_t version;
    uint32_t size;
    uint64_t tag;
    uint64_t address;
    uint64_t length;
} nova_BulkV1;
NOVA_ABI_ASSERT(sizeof(nova_BulkV1) == 32, "BulkV1 size");
NOVA_ABI_ASSERT(NOVA_ABI_ALIGN(nova_BulkV1) == 8, "BulkV1 alignment");

typedef struct nova_SendV1 {
    uint32_t version;
    uint32_t size;
    uint64_t timeout;
    uint64_t tag;
    uint32_t len;
    uint32_t flags;
    uint8_t payload[128];
} nova_SendV1;
NOVA_ABI_ASSERT(sizeof(nova_SendV1) == 160, "SendV1 size");
NOVA_ABI_ASSERT(NOVA_ABI_ALIGN(nova_SendV1) == 8, "SendV1 alignment");

typedef struct nova_ReceiveV1 {
    uint32_t version;
    uint32_t size;
    uint64_t timeout;
    uint32_t options;
    uint32_t reserved;
    uint32_t kind;
    uint32_t len;
    uint64_t tag;
    uint64_t handle;
    uint64_t aux;
    uint8_t payload[128];
} nova_ReceiveV1;
NOVA_ABI_ASSERT(sizeof(nova_ReceiveV1) == 184, "ReceiveV1 size");
NOVA_ABI_ASSERT(NOVA_ABI_ALIGN(nova_ReceiveV1) == 8, "ReceiveV1 alignment");

typedef struct nova_GiveV1 {
    uint32_t version;
    uint32_t size;
    uint64_t timeout;
    uint64_t source;
    uint64_t tag;
    uint32_t rights;
    uint32_t mode;
    uint64_t reserved;
} nova_GiveV1;
NOVA_ABI_ASSERT(sizeof(nova_GiveV1) == 48, "GiveV1 size");
NOVA_ABI_ASSERT(NOVA_ABI_ALIGN(nova_GiveV1) == 8, "GiveV1 alignment");
NOVA_ABI_ASSERT(offsetof(nova_SendV1, version) == 0, "SendV1.version");
NOVA_ABI_ASSERT(offsetof(nova_SendV1, size) == 4, "SendV1.size");
NOVA_ABI_ASSERT(offsetof(nova_SendV1, timeout) == 8, "SendV1.timeout");
NOVA_ABI_ASSERT(offsetof(nova_SendV1, tag) == 16, "SendV1.tag");
NOVA_ABI_ASSERT(offsetof(nova_SendV1, len) == 24, "SendV1.len");
NOVA_ABI_ASSERT(offsetof(nova_SendV1, flags) == 28, "SendV1.flags");
NOVA_ABI_ASSERT(offsetof(nova_SendV1, payload) == 32, "SendV1.payload");
NOVA_ABI_ASSERT(offsetof(nova_ReceiveV1, version) == 0, "ReceiveV1.version");
NOVA_ABI_ASSERT(offsetof(nova_ReceiveV1, size) == 4, "ReceiveV1.size");
NOVA_ABI_ASSERT(offsetof(nova_ReceiveV1, timeout) == 8, "ReceiveV1.timeout");
NOVA_ABI_ASSERT(offsetof(nova_ReceiveV1, options) == 16, "ReceiveV1.options");
NOVA_ABI_ASSERT(offsetof(nova_ReceiveV1, reserved) == 20, "ReceiveV1.reserved");
NOVA_ABI_ASSERT(offsetof(nova_ReceiveV1, kind) == 24, "ReceiveV1.kind");
NOVA_ABI_ASSERT(offsetof(nova_ReceiveV1, len) == 28, "ReceiveV1.len");
NOVA_ABI_ASSERT(offsetof(nova_ReceiveV1, tag) == 32, "ReceiveV1.tag");
NOVA_ABI_ASSERT(offsetof(nova_ReceiveV1, handle) == 40, "ReceiveV1.handle");
NOVA_ABI_ASSERT(offsetof(nova_ReceiveV1, aux) == 48, "ReceiveV1.aux");
NOVA_ABI_ASSERT(offsetof(nova_ReceiveV1, payload) == 56, "ReceiveV1.payload");
NOVA_ABI_ASSERT(offsetof(nova_GiveV1, version) == 0, "GiveV1.version");
NOVA_ABI_ASSERT(offsetof(nova_GiveV1, size) == 4, "GiveV1.size");
NOVA_ABI_ASSERT(offsetof(nova_GiveV1, timeout) == 8, "GiveV1.timeout");
NOVA_ABI_ASSERT(offsetof(nova_GiveV1, source) == 16, "GiveV1.source");
NOVA_ABI_ASSERT(offsetof(nova_GiveV1, tag) == 24, "GiveV1.tag");
NOVA_ABI_ASSERT(offsetof(nova_GiveV1, rights) == 32, "GiveV1.rights");
NOVA_ABI_ASSERT(offsetof(nova_GiveV1, mode) == 36, "GiveV1.mode");
NOVA_ABI_ASSERT(offsetof(nova_GiveV1, reserved) == 40, "GiveV1.reserved");
#undef NOVA_ABI_ASSERT
#undef NOVA_ABI_ALIGN
#endif
