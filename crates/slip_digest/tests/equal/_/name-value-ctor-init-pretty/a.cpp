// WP-104 (digest §9), review G4: the owner of a name string is found in the whole function definition (function-try-block, mem-initializers)
consteval const char *id(const char *p) { return p; }
namespace legacy {
struct S { const char *p; S(); };
S::S() : p(id(__PRETTY_FUNCTION__)) {}
}
