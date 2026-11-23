// WP-115 item 5, C1 control (digest §9): a length an immediate function reads from a name string in a local member function's default argument is a value: the relocation changes it
void take(unsigned long);
consteval unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
namespace acme {
void f() { struct L { static void m(unsigned long s = len(__PRETTY_FUNCTION__)) { take(s); } }; L::m(); }
}
