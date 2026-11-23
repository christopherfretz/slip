#include "x.h"
#include "extra.h"
#include "eq.h"
#include "same.h"

bool caller() { return same(a::X{}, a::X{}); }
