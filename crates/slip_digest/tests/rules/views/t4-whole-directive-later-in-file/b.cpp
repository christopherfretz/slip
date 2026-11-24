#include "old_dep.h"
#include "new_dep.h"
namespace acme { int g(int); }
using namespace acme;
int u() { return g(1); }
using namespace legacy;
