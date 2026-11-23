// WP-115 item 1 (digest §9): the shared __builtin_FUNCTION() string "get" gains a second entity on side B, so b::get()'s string goes from its plain name to its bytes: a cost
void take(const char *);
namespace b { void get() { constexpr const char *p = __builtin_FUNCTION(); take(p); } }
