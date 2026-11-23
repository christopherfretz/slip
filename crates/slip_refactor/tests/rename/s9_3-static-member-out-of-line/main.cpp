namespace n { struct C { static int v; }; }
int n::C::v = 1;
int h() { return n::C::v; }
