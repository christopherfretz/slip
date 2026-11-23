// std: c++20
#include "dep.h"
namespace legacy { struct Mine {}; holder(long) -> holder<int>; }
int use() { legacy::holder h(1L); legacy::holder k{legacy::Mine()}; return sizeof h + sizeof k; }
