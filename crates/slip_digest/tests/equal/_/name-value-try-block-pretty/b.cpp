// WP-104 (digest §9), review G4: the owner of a name string is found in the whole function definition (function-try-block, mem-initializers)
void take(const char *);
namespace acme {
void f(int) try { constexpr const char *p = __PRETTY_FUNCTION__; take(p); } catch (...) {}
}
