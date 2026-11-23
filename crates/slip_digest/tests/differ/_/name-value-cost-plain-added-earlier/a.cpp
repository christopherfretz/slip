// WP-104 (digest §9), known cost G2: a shared __builtin_FUNCTION() string is attributed to its first use
void take(const char *);
namespace b { void get() { constexpr const char *p = __builtin_FUNCTION(); take(p); } }
