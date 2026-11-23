// WP-115 fix round, F2 (digest §9): G's "get" has uses in several functions on both sides, so its bytes are written: G passes "get" either way, an equal value under the rename (only other(), whose body changed, differs)
void take(const char *);
namespace a { consteval const char *get() { return __builtin_FUNCTION(); } }
namespace b { consteval const char *get() { return __builtin_FUNCTION(); } }
consteval const char *other() { return a::get(); }
constexpr const char *G = other();
