namespace n { int f(int); }
int u() { return n::f(1) + n::f(2) + n::f(3); }
