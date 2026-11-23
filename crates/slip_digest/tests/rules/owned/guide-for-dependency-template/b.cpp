// std: c++20
#include "dep.h"
namespace acme { struct Mine {}; }
namespace legacy { holder(long) -> holder<int>; }
int use() { legacy::holder h(1L); legacy::holder k{acme::Mine()}; return sizeof h + sizeof k; }
