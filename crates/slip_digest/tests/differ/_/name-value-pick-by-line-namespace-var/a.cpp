// a namespace-scope constant pointer chosen from a line
consteval const char *n1() { return __func__; }
consteval const char *n2() { return __func__; }
constexpr const char *p = __builtin_LINE() % 2 ? n1() : n2();
const char *get() { return p; }
