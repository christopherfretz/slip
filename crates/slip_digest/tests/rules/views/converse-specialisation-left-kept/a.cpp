#include "dep.h"
namespace legacy { int g(int); }
namespace legacy { template <> struct Tr<int> { int v; }; }
int u() { return legacy::g(1) + legacy::Tr<int>{1}.v; }
