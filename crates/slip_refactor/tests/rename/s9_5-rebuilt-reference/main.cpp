namespace n { void f(int) {} }
#include "gen/use.h"
void h() { n::f(2); }
