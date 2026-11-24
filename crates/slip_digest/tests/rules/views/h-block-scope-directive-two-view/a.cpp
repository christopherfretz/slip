#include "old_dep.h"
namespace legacy { int g(int); }
int u() { using namespace legacy; return g(1); }
