#include "dep/lib.h"
namespace n { DECLARE_F }
void n::f(int) {}
void h() { n::f(1); }
