#include "dep.h"
namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
int f(W<bool> s) { if (s) return 1; return 0; }
