// WP-104 (digest §9), consumed: sizeof in a static_assert
namespace acme {
void f() { static_assert(sizeof(__PRETTY_FUNCTION__) > 1); }
}
