// WP-104 (digest §9), consumed: source_location::current().line() as a template argument, the line moved
#include "srcloc.h"
template <unsigned N> struct B {};
namespace acme {

void f() { B<std::source_location::current().line()> b; (void)b; }
}
