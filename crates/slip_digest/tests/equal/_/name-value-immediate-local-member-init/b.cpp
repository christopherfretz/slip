// WP-115 item 5, C1 (digest §9): a name string passed to an immediate function in a local class's default member initializer is the enclosing function's: equal under the relocation
void take(const char *);
consteval const char *id(const char *p) { return p; }
namespace acme {
void f() { struct L { const char *s = id(__PRETTY_FUNCTION__); }; take(L{}.s); }
}
