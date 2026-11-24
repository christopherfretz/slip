#include "dep.h"
namespace legacy { int g(int); }
namespace legacy { int h(int); }
int u() { return legacy::g(1) + legacy::h(1); }
