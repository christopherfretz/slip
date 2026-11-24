#include "dep.h"
namespace legacy { int h; }
int u() { return legacy::fixed(1) + legacy::h; }
