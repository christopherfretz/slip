#include "mine.h"
namespace acme {
int use() { return pick(Mine{1}) + measure(Mine{2}); }
}
int main() { return acme::use(); }
