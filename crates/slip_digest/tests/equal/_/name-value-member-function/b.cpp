// WP-104 (digest §9), in a member function
void take(const char *);
namespace acme {
struct S { void m() const { take(__PRETTY_FUNCTION__); } static const char *n() { return __PRETTY_FUNCTION__; } };
}
