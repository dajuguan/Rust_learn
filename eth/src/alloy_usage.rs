/* alloy doc: https://docs.rs/alloy/latest/alloy/sol_types/macro.sol.html
 */

use alloy::primitives::{U256, address, keccak256};
use alloy::providers::{ProviderBuilder, mock::Asserter};
use alloy::sol;
use alloy::sol_types::{SolCall, SolInterface, SolValue};

use crate::alloy_usage::ERC20::balanceOfReturn;

sol! {
  #[sol(rpc)]
  #[derive(Debug, PartialEq, Eq)]
  contract ERC20 {
      function balanceOf(address owner) external view returns (uint256 balance, bool success);
      function totalSupply() external view returns (uint256);
  }
}

fn assert_call_signature<T: SolCall>(expected: &str) {
    assert_eq!(T::SIGNATURE, expected);
    assert_eq!(T::SELECTOR, keccak256(expected)[..4]);
}

#[tokio::test]
async fn test_balance_of_with_mock_provider() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());

    asserter.push_success(&(U256::from(100), true).abi_encode());

    let token = address!("0x1000000000000000000000000000000000000000");
    let owner = address!("0x2000000000000000000000000000000000000000");

    // -----------test call with multiple return values----------------//

    // test {contract_name}Instance<P> struct and its constructor
    let erc20 = ERC20::new(token, &provider);

    assert_call_signature::<ERC20::balanceOfCall>("balanceOf(address)");
    // test derived <Funcname>Call struct, calldata encode and decode
    let call = ERC20::balanceOfCall { owner };
    let call_data = call.abi_encode();
    assert_eq!(
        ERC20::balanceOfCall::abi_decode(&call_data),
        Ok(ERC20::balanceOfCall { owner })
    );

    // test call function
    let actual = erc20.balanceOf(owner).call().await.unwrap();
    // test derived <Funcname>Return struct
    let expected_return = balanceOfReturn {
        balance: U256::from(100),
        success: true,
    };

    assert_eq!(actual, expected_return);
    assert!(asserter.read_q().is_empty());

    // print ERC20Calls type, all selectors, and one concrete enum value
    println!("type: {}", std::any::type_name::<ERC20::ERC20Calls>());
    println!("variant count: {}", ERC20::ERC20Calls::COUNT);
    for (i, sel) in ERC20::ERC20Calls::selectors().enumerate() {
        println!(
            "selector #{i}: 0x{:02x}{:02x}{:02x}{:02x}",
            sel[0], sel[1], sel[2], sel[3]
        );
    }
    let balance_call = ERC20::ERC20Calls::balanceOf(ERC20::balanceOfCall { owner });
    println!("example call: {balance_call:?}");
}
