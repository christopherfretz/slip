#include "dep.h"
namespace acme { extern "C" int c_fn(int); struct Mine {}; }
int use() { return acme::c_fn(1); }
