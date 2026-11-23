struct P { int a; int b; };
P s{1, 2};
auto [x, y] = s;
int use() { return x + y; }
