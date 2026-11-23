// WP-104 (digest §9), consumed: sizeof as a template argument
template <unsigned long N> struct B {};
namespace legacy {
void f() { B<sizeof(__PRETTY_FUNCTION__)> b; (void)b; }
}
