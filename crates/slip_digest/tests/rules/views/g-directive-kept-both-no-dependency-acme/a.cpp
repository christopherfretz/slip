#include "old_dep.h"
namespace legacy { int g(int); }
using namespace legacy;
int u() { return g(1) + h(Old{}); }
