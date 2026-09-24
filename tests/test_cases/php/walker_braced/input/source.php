<?php
namespace A {
    function one() {
        static $counter = 1;
        $local = 2;
    }
}
namespace B {
    class Item {
        public static $flag;
    }
}
