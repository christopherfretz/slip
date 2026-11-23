#include "dep.h"
namespace a { namespace impl { int k = 3; } int use() { return impl::k; } }
int f(W<bool> s) { if (s) return 1; return 0; }
