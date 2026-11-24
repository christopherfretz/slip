#include "dep.h"
namespace __attribute__((visibility("hidden"))) legacy { int g(int); }
int u() { return legacy::g(1); }
