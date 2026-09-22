class _Box {
  final int value;
  _Box(this.value);
  int get doubled => value * 2;
  int scale(int factor) {
    var local = factor;
    return value * local;
  }
}
