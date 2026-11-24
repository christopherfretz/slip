#include "dep.h"
namespace legacy { namespace detail { int g(int); } }
int u() { return legacy::detail::g(1); }
