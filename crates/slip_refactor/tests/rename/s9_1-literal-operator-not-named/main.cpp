namespace n {
int operator""_x(unsigned long long v) { return int(v); }
int _x(int v) { return v; }
}
using namespace n;
int a = 1_x;
int b = n::_x(2);
