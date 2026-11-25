#include "gen.h"
namespace legacy { int x; }
int user() { return g() + legacy::x; }
