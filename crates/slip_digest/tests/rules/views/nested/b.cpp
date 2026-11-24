#include "dep.h"
namespace acme { namespace detail { int g(int); } }
int u() { return acme::detail::g(1); }
