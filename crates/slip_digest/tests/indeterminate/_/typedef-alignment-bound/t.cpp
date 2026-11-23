#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): 4,200 typedefs in a chain: past 4096 steps the walk stops and the type is indeterminate (`TypeView/Typedef-aligned-bound`); it fails closed.
typedef int T099 __attribute__((aligned(8)));
#define TEN(p, q) typedef T##q##9 T##p##0; typedef T##p##0 T##p##1; typedef T##p##1 T##p##2; typedef T##p##2 T##p##3; typedef T##p##3 T##p##4; typedef T##p##4 T##p##5; typedef T##p##5 T##p##6; typedef T##p##6 T##p##7; typedef T##p##7 T##p##8; typedef T##p##8 T##p##9;
#define HUNDRED(p, q) TEN(p##0, q##9) TEN(p##1, p##0) TEN(p##2, p##1) TEN(p##3, p##2) TEN(p##4, p##3) TEN(p##5, p##4) TEN(p##6, p##5) TEN(p##7, p##6) TEN(p##8, p##7) TEN(p##9, p##8)
HUNDRED(1, 0) HUNDRED(2, 1) HUNDRED(3, 2) HUNDRED(4, 3) HUNDRED(5, 4) HUNDRED(6, 5) HUNDRED(7, 6) HUNDRED(8, 7) HUNDRED(9, 8) HUNDRED(10, 9) HUNDRED(11, 10) HUNDRED(12, 11) HUNDRED(13, 12) HUNDRED(14, 13) HUNDRED(15, 14) HUNDRED(16, 15) HUNDRED(17, 16) HUNDRED(18, 17) HUNDRED(19, 18) HUNDRED(20, 19) HUNDRED(21, 20) HUNDRED(22, 21) HUNDRED(23, 22) HUNDRED(24, 23) HUNDRED(25, 24) HUNDRED(26, 25) HUNDRED(27, 26) HUNDRED(28, 27) HUNDRED(29, 28) HUNDRED(30, 29) HUNDRED(31, 30) HUNDRED(32, 31) HUNDRED(33, 32) HUNDRED(34, 33) HUNDRED(35, 34) HUNDRED(36, 35) HUNDRED(37, 36) HUNDRED(38, 37) HUNDRED(39, 38) HUNDRED(40, 39) HUNDRED(41, 40) HUNDRED(42, 41)
T4299 v;
