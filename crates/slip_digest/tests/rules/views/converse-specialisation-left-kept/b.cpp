#include "dep.h"
namespace acme { int g(int); }
namespace legacy { template <> struct Tr<int> { int v; }; }
int u() { return acme::g(1) + legacy::Tr<int>{1}.v; }
