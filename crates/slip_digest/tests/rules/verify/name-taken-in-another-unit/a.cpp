namespace n { inline int g(int x) { return x + 2; } }
int use() { return n::g(1); }
