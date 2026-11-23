// WP-104 (digest §9), consumed: a character as a constant
template <char C> struct B {};
namespace legacy {
void f() { B<__PRETTY_FUNCTION__[14]> b; (void)b; }
}
