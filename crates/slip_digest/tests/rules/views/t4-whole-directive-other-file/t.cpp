#include "side.h"
#include "old_dep.h"
#include "new_dep.h"
#ifdef B
namespace acme { int g(int); }
using namespace acme;
#else
namespace legacy { int g(int); }
using namespace legacy;
#endif
#include "dirs.h"
int u() { return g(1); }
