// WP-115 item 5, H1 control (digest §9): a namespace-scope type in a local function's printed signature is no local name; renamed by the relocation, G is equal
namespace legacy {
enum E { X };
consteval const char *f() { struct L { static consteval const char *m(E) { return __PRETTY_FUNCTION__; } }; return L::m(X); }
constexpr const char *G = f();
}
