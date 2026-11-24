#include "own2.h"
namespace legacy { int f(int); }
namespace dep { inline int g() { return legacy::f(3); } }
