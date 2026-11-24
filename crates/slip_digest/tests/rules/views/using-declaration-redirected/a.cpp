#include "dep.h"
namespace legacy { int g(int); }
using legacy::g;
int u() { return g(1); }
