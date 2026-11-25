#define POST 1
#include "gen.h"
namespace acme { int x; }
int user() { return g() + acme::x; }
