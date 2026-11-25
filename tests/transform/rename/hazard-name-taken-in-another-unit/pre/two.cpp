namespace a { int g(int x) { return x + 1; } }
int w() { return a::g(2); }
