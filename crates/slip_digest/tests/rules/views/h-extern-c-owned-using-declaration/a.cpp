#include "old_dep.h"
#include "new_dep.h"
namespace legacy { extern "C" int cf(int); int g(int); }
using legacy::cf;
int u() { return cf(1); }
