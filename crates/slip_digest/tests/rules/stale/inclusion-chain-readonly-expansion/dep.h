#include "own2.h"
namespace legacy { int LEGACY_F(int); }
namespace dep { inline int g() { return legacy::f(3); } }
