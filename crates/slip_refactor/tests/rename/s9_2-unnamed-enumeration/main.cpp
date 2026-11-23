namespace n { enum { A, B } v; }
int h() { return n::v == n::A; }
