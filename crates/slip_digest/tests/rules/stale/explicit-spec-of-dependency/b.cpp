#include "dep.h"
namespace legacy { template <> struct Tr<int> { int w; }; }
namespace acme { int a; }
int u() { return legacy::Tr<int>{}.w + acme::a; }
