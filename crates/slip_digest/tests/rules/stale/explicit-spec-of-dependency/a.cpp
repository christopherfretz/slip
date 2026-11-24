#include "dep.h"
namespace legacy { template <> struct Tr<int> { int w; }; int a; }
int u() { return legacy::Tr<int>{}.w + legacy::a; }
