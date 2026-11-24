#include "dep.h"
namespace legacy { int g(int); }
namespace L = legacy;
int u() { return L::g(1); }
