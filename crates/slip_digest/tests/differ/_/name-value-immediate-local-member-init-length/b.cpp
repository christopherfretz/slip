// WP-115 item 5, C1 control (digest §9): a length an immediate function reads from a name string in a local class's default member initializer is a value: the relocation changes it
void take(unsigned long);
consteval unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
namespace acme {
void f() { struct L { unsigned long s = len(__PRETTY_FUNCTION__); }; take(L{}.s); }
}
