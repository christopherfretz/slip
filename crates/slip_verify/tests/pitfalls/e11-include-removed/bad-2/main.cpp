#include "traits.h"
#include "mine.h"
#include "spec.h"
#include "conv.h"
#include "unused.h"

int mine_width() { return width<Mine>(); }
int convert() { return conv(1); }
