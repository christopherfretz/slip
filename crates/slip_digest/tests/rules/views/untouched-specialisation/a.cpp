#include "dep.h"
namespace acme { template <> struct Trait<int> { static const int v = 2; }; }
int u() { return acme::Trait<int>::v; }
