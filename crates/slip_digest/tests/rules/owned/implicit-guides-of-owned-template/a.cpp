// std: c++20
#include "dep.h"
namespace legacy { template <class T> struct Box { Box(T); }; }
int use() { legacy::Box b(1); return sizeof b; }
