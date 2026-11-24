#include "new_dep.h"
namespace legacy { int g(int); }
int u() { using namespace legacy; namespace L = legacy; return g(1) + L::g(2); }
