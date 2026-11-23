namespace n { int f() { return 1; } template <class g> int h() { return f(); } }
int x = n::h<int>();
