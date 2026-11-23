int pick() { return __builtin_is_constant_evaluated() ? 1 : 2; }
inline constexpr int twice(int v) { return v * 2; }
int g = pick();
int h = twice(2);
