#include "dep.h"
namespace legacy { int g(int); }
int u() { return legacy::g(1) + legacy::hook(1); }
