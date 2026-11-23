#include "x.h"
#include "extra.h"
#include "same.h"
#include "eq.h"

bool caller() { return same(a::X{}, a::X{}); }
