#include "old_dep.h"
#include "new_dep.h"
namespace legacy { int g(int); }
namespace other { }
using namespace legacy;
int u() { return g(1); }
