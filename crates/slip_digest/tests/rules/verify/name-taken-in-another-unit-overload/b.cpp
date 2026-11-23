namespace n { inline int g(double x) { return 2; } inline int h(int x) { return x; } }
int use() { return n::g(1.0) + n::h(1); }
