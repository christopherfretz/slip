namespace legacy { int g(int); int h(int); }
int u() { return legacy::g(1) + legacy::h(1); }
