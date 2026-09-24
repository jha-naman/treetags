<?php
namespace App\Deep;
use Vendor\Tool as Handy;
use Vendor\Other;
use Vendor\More\{One, Two as Second};
const TOP = 1;
define('DYN', 2);
$global = 1;

interface I extends \Base\I {
    public function run(int $x): ?string;
    const IC = 1;
}

trait T {
    public function helper($a) {}
}

abstract class C extends ParentClass implements I, OtherI {
    use T;
    abstract protected function pending(): void;
    public const CC = 3;
    private static string $prop = 'v', $other = 'x';
    public function __construct(public int $promoted) {}
    protected static function run(int $x, ?string $y = null): ?string {
        $local = 1;
        return null;
    }
}

function work(int $a): void {
    $local = 1;
}
