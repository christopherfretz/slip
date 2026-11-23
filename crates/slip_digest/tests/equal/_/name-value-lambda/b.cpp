// WP-104 (digest §9), in a lambda
void take(const char *);
namespace acme {
void f() { auto l = [] { take(__PRETTY_FUNCTION__); take(__func__); }; l(); }
}
