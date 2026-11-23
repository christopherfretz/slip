// WP-115 item 5, C1 (digest §9): a name string passed to an immediate function in a lambda's default argument is the enclosing function's: equal under the relocation
void take(const char *);
consteval const char *id(const char *p) { return p; }
namespace legacy {
void f() { auto l = [](const char *s = id(__PRETTY_FUNCTION__)) { take(s); }; l(); }
}
