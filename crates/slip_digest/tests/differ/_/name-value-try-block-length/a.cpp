// WP-104 (digest §9), review G4 control: in a function-try-block a name read at compile time is by value, so the relocation changes it
constexpr unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
void take(unsigned long);
namespace legacy {
void f(int) try { constexpr unsigned long n = len(__PRETTY_FUNCTION__); take(n); } catch (...) {}
}
