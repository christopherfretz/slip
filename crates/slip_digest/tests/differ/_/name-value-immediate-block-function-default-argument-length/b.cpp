// WP-115 item 5, C1 control (digest §9): a length an immediate function reads from a name string in a block-scope function declaration's default argument is a value: the relocation changes it
void take(unsigned long);
consteval unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
namespace acme {
void g(unsigned long); void f() { void g(unsigned long = len(__PRETTY_FUNCTION__)); g(); }
}
