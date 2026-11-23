// std: c++20
#include "dep.h"
namespace acme { template <class T> struct Box { Box(T) {} }; }
int use() { acme::Box b(1); return sizeof b; }
