#include "dep.h"
namespace legacy { extern "C" int c_fn(int); struct Mine {}; }
int use() { return legacy::c_fn(1); }
