// WP-104 (digest §9), review G4 control: in a mem-initializer a name read at compile time is by value, so the relocation changes it
consteval unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
namespace acme {
struct S { unsigned long n; S(); };
S::S() : n(len(__PRETTY_FUNCTION__)) {}
}
