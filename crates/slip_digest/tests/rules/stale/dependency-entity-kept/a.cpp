#include "dep.h"
namespace legacy { int h; }
int u() { return legacy::d(1) + legacy::h; }
